//! Seems to be buggy. Don't use it.

use crate::okvs::OkvsDecoder;
use crate::okvs::OkvsEncoder;
use crate::hash::Hashable;
use crate::Block;

type SnapBlock = u64;
const SNAP_LEN: usize = 64;

const DEBUG: bool = true;

use crate::utils::xor_u64s_inplace;
use crate::utils::dot_u64_generic;

#[derive(Clone, Debug)]
pub struct BPSY23 {
    /// if input `n` key-value pairs, will produce a encoded vector of
    /// length `m = ceil(n * (1 + epsilon))`
    epsilon: f64,
    /// Matrix non-zero band width
    width: usize,
}

#[inline]
fn hash_row_k<T>(key: &T, count: usize) -> (usize, Vec<SnapBlock>) where T: Hashable + std::any::Any {
    let mut hash = key.hash_to_hasher().finalize_xof();
    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<Block>() {
        let key = unsafe {*(key as *const T as *const Block)};
        let required_bytes = 8 + count * std::mem::size_of::<SnapBlock>();
        let required_blocks = (required_bytes + 15) / 16;
        let mut buf = vec![Block::default(); required_blocks];
        for i in 0..required_blocks {
            buf[i] = Block(key.0.wrapping_add(i as u128)).hash_to_block();
        }
        unsafe {
            // take the start 8 bytes of buf
            let buf0 = std::slice::from_raw_parts(
                buf.as_ptr() as *const u8,
                std::mem::size_of::<usize>()
            );
            // take the latter count * 8 bytes of buf
            let buf1 = std::slice::from_raw_parts(
                (buf.as_ptr() as *const u8).add(8),
                count * std::mem::size_of::<SnapBlock>()
            );
            let mut start_index = 0;
            std::slice::from_raw_parts_mut(
                &mut start_index as *mut usize as *mut u8,
                std::mem::size_of::<usize>()
            ).copy_from_slice(buf0);
            let mut offsets = vec![0 as SnapBlock; count];
            std::slice::from_raw_parts_mut(
                offsets.as_mut_ptr() as *mut u8,
                count * std::mem::size_of::<SnapBlock>()
            ).copy_from_slice(buf1);
            (start_index, offsets)
        }
    } else {
        let mut start_index: usize = 0;
        unsafe {
            hash.fill(std::slice::from_raw_parts_mut(
                &mut start_index as *mut usize as *mut u8,
                std::mem::size_of::<usize>()
            ));
        }
        start_index %= count * SNAP_LEN;
        let mut offsets = vec![0 as SnapBlock; count];
        unsafe {
            hash.fill(std::slice::from_raw_parts_mut(
                offsets.as_mut_ptr() as *mut u8,
                count * std::mem::size_of::<SnapBlock>()
            ));
        }
        (start_index, offsets)
    }
}

fn row_k<Key>(key: &Key, m: usize, width: usize) -> (usize, Vec<SnapBlock>) where Key: Hashable + std::any::Any {
    let count = (width - 2 + SNAP_LEN) / SNAP_LEN + 1;
    let (mut start_index, mut offsets) = hash_row_k(key, count);
    start_index %= m - width;
    offsets[0] &= !((1 << (start_index % SNAP_LEN)) - 1);
    let last_index = ((start_index % SNAP_LEN) + width) / SNAP_LEN;
    assert!(last_index >= count - 2);
    if last_index < count {
        offsets[last_index] &= (1 << ((start_index + width) % SNAP_LEN)) - 1;
    }
    if last_index == count - 2 {
        offsets[last_index + 1] = 0;
    }
    (start_index, offsets)
}

impl BPSY23 {

    pub fn new(epsilon: f64, width: usize) -> Self {
        Self { epsilon, width }
    }
    
    #[allow(unused)]
    fn encode_length(&self, count: usize) -> usize {
        let m = (count as f64 * (1.0 + self.epsilon)).ceil() as usize;
        m
    }

}

impl<Key, Value> OkvsEncoder<Key, Value> for BPSY23 where
    Key: Hashable + std::any::Any,
    Value: Default + Clone + From<SnapBlock> + std::ops::Mul<Output=Value> + std::ops::BitXorAssign
{

    fn encode(&self, map: &Vec<(Key, Value)>) -> Vec<Value> {
        use crate::utils::TimerOnce;

        // sanity
        let n = map.len();
        let m = (n as f64 * (1.0 + self.epsilon)).ceil() as usize;
        assert!(m > self.width);

        // construct matrix M
        let timer = TimerOnce::new().tabs(2);
        let mut rows = Vec::with_capacity(n);
        for (key, value) in map {
            let (start_index, offsets) = row_k(key, m, self.width);
            rows.push((start_index, offsets, value.clone()));
        }
        if DEBUG {timer.finish("Construct matrix");}

        // Sort with first index
        let timer = TimerOnce::new().tabs(2);
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        if DEBUG {timer.finish("Sort");}

        // Gaussian elimination solving M * s = v
        // see https://arxiv.org/pdf/1907.04750.pdf
        // Efficient Gauss Elimination for Near-Quadratic Matrices with One Short Random Block per Row, with Applications
        let timer = TimerOnce::new().tabs(2);
        let mut offsets = Vec::with_capacity(n);
        let mut v = Vec::with_capacity(n);
        let mut start_indices = Vec::with_capacity(n);
        for (start_index, offset, value) in rows {
            start_indices.push(start_index);
            v.push(value);
            offsets.push(offset);
        }
        if DEBUG {timer.finish("List");}

        // let print_row = |start_index: usize, offsets: &[u64]| -> String {
        //     let start_index = start_index & !(SNAP_LEN - 1);
        //     let mut out = String::new();
        //     for _i in 0..start_index {out.push('0');}
        //     for i in 0..offsets.len() {
        //         for j in 0..SNAP_LEN {
        //             if start_index + i * SNAP_LEN + j >= m {
        //                 break;
        //             }
        //             if offsets[i] & (1 << j) != 0 {
        //                 out.push('1');
        //             } else {
        //                 out.push('0');
        //             }
        //         }
        //     }
        //     if start_index + offsets.len() * SNAP_LEN < m {
        //         let remaining = m - start_index - offsets.len() * SNAP_LEN;
        //         for _i in 0..remaining {out.push('0');}
        //     }
        //     out  
        // };

        // println!("Initial matrix:");
        // for i in 0..n {
        //     println!("i={:02}, {}", i, print_row(start_indices[i], &offsets[i]));
        // }

        let timer = TimerOnce::new().tabs(2);
        for i in 0..n {
            // println!("i={:02}", i);
            let i_id = start_indices[i] / SNAP_LEN;
            let mut j = 0;
            let mut found = false;
            for each in &offsets[i] {
                if *each != 0 {
                    found = true;
                    j += each.trailing_zeros() as usize;
                    break;
                }
                j += SNAP_LEN;
            }
            if !found {
                panic!("Matrix is singular");
            }
            for k in (i + 1)..n {
                if start_indices[k] > i_id * SNAP_LEN + j {
                    break;
                }
                let k_id = start_indices[k] / SNAP_LEN;
                let id_offset = k_id - i_id;
                if (offsets[k][j / SNAP_LEN - id_offset] >> (j % SNAP_LEN)) & 1 != 0 {
                    // xor row i from row k
                    // println!("substract row {} from row {}, i_id={}, k_id={}, klen={}", i, k, i_id, k_id, offsets[k].len());
                    // println!("before row[{:2}] = {}", i, print_row(start_indices[i], &offsets[i]));
                    // println!("before row[{:2}] = {}", k, print_row(start_indices[k], &offsets[k]));
                    let vi = v[i].clone();
                    v[k] ^= vi;
                    unsafe {xor_u64s_inplace(
                        offsets[k].as_mut_ptr(), 
                        offsets[i].as_ptr().add(id_offset), 
                        offsets[k].len() - id_offset
                    );}
                }
            }
        }
        if DEBUG {timer.finish("Gaussian elimination");}

        // println!("Final matrix:");
        // for i in 0..n {
        //     println!("i={:02}, {}", i, print_row(start_indices[i], &offsets[i]));
        // }

        // Reverse solve
        let timer = TimerOnce::new().tabs(2);
        let mut s = vec![Value::default(); m];
        for i in (0..n).rev() {
            let mut j = 0;
            for each in &offsets[i] {
                if *each != 0 {
                    j += each.trailing_zeros() as usize;
                    break;
                }
                j += SNAP_LEN;
            }
            let mut sum = v[i].clone();
            let i_id = start_indices[i] / SNAP_LEN;
            for k in 0..offsets[i].len() {
                if (i_id + k) * SNAP_LEN >= s.len() {
                    continue;
                }
                let range = &s[(i_id + k) * SNAP_LEN..];
                sum ^= dot_u64_generic(offsets[i][k], range);
            }
            s[i_id * SNAP_LEN + j] = sum;
        }
        if DEBUG {timer.finish("Reverse solve");}
        s
    }
}

impl<Key, Value> OkvsDecoder<Key, Value> for BPSY23 where
    Key: Hashable + std::any::Any,
    Value: Default + Clone + From<SnapBlock> + std::ops::Mul<Output=Value> + std::ops::BitXorAssign
{
    fn decode(&self, okvs: &[Value], key: &Key) -> Value {
        let (start_index, offsets) = row_k(key, okvs.len(), self.width);
        let mut sum = Value::default();
        let i_id = start_index / SNAP_LEN;
        for k in 0..offsets.len() {
            if (i_id + k) * SNAP_LEN >= okvs.len() {
                continue;
            }
            let range = &okvs[(i_id + k) * SNAP_LEN..];
            sum ^= dot_u64_generic(offsets[k], range);
        }
        sum
    }
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use crate::Block;

    #[test]
    pub fn bpsy23_encode() {
        let mut map = Vec::new();
        let n: usize = 200;
        let width: usize = 87;
        let keys = (0..n).collect::<Vec<_>>();
        for &i in &keys {
            map.push((i, Block((i*i) as u128)));
        }
        let encoder = BPSY23::new(0.03, width);
        let s = encoder.encode(&map);
        for (key, value) in map {
            assert_eq!(encoder.decode(&s, &key), value, "key = {}", key);
        }
    }

}
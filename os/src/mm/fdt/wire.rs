//! FDT 大端整数、字符串和四字节对齐的边界检查。

pub(super) fn be32(data: &[u8], offset: usize) -> Result<u32, &'static str> {
    let end = offset.checked_add(4).ok_or("FDT offset overflow")?;
    let bytes = data.get(offset..end).ok_or("truncated FDT word")?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

pub(super) fn be64(data: &[u8], offset: usize) -> Result<u64, &'static str> {
    Ok(((be32(data, offset)? as u64) << 32) | be32(data, offset + 4)? as u64)
}

pub(super) fn cstring(data: &[u8], start: usize) -> Result<(&[u8], usize), &'static str> {
    let tail = data.get(start..).ok_or("invalid FDT string offset")?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or("unterminated FDT string")?;
    Ok((&tail[..length], start + length + 1))
}

pub(super) fn align4(value: usize) -> Result<usize, &'static str> {
    Ok(value.checked_add(3).ok_or("FDT alignment overflow")? & !3)
}

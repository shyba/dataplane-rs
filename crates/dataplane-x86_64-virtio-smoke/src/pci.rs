use crate::io::{inl, outl};

const CONFIG_ADDRESS: u16 = 0xcf8;
const CONFIG_DATA: u16 = 0xcfc;

pub fn read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc);
    outl(CONFIG_ADDRESS, address);
    inl(CONFIG_DATA)
}

pub fn read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let value = read_u32(bus, device, function, offset);
    let shift = u32::from(offset & 0x02) * 8;
    ((value >> shift) & 0xffff) as u16
}

pub fn write_u16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let mut current = read_u32(bus, device, function, offset);
    let shift = u32::from(offset & 0x02) * 8;
    current &= !(0xffff << shift);
    current |= u32::from(value) << shift;
    let address = 0x8000_0000u32
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc);
    outl(CONFIG_ADDRESS, address);
    outl(CONFIG_DATA, current);
}

//! Where Matter saves what it must remember: the flash's NVS partition
//! (step 4c, ADR-0011).

use embassy_embedded_hal::adapter::BlockingAsync;
use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType,
};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use log::{info, warn};
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// Opens the flash's NVS partition as a key-value store for Matter.
///
/// The partition table (written by `espflash` next to the program) says
/// where NVS is; with the default table, 24 KB at 0x9000. Reading it
/// instead of hard-coding the address keeps working if the table changes.
///
/// With `erase`, the partition is wiped first: the lamp forgets its pairing
/// and its last state, as on its very first start.
#[allow(
    clippy::large_stack_frames,
    reason = "called once from `run`, on main's stack (same exemption); the 3 KB table \
    buffer is freed as soon as the NVS partition is found"
)]
pub(super) fn flash_store(
    flash: FLASH<'static>,
    erase: bool,
) -> SeqMapKvBlobStore<BlockingAsync<FlashStorage<'static>>> {
    let mut flash = FlashStorage::new(flash);

    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let nvs = partitions::read_partition_table(&mut flash, &mut table)
        .expect("partition table")
        .find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
        .expect("partition table")
        .expect("no NVS partition in the partition table");
    let range = nvs.offset()..nvs.offset() + nvs.len();
    info!("Matter: saving to the NVS partition ({range:#x?})");

    if erase {
        warn!("Matter: erasing the NVS partition (pairing forgotten)");
        flash
            .erase(range.start, range.end)
            .expect("erasing the NVS partition");
    }

    // The store is written for async flash; ours is blocking (each write
    // waits for the chip), `BlockingAsync` adapts one to the other.
    SeqMapKvBlobStore::new(BlockingAsync::new(flash), range)
}

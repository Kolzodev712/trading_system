from indicator_config_generator.builder import sma
from indicator_config_generator.mongo import simple_entry_to_mongo_doc


def test_simple_mongo_doc_matches_rust_debug_source() -> None:
    doc = simple_entry_to_mongo_doc(sma("BTCUSDT", 20), now=1_700_000_000)
    assert doc["_id"] == "sma_20_BTCUSDT"
    assert doc["indicator_type"] == "simple"
    assert doc["source"] == "Close"
    assert doc["period"] == 20

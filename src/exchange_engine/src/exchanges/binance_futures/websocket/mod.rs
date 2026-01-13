//! Binance Futures WebSocket Client
//!
//! This module contains the WebSocket client implementation for Binance USD Futures.
//! Note: Binance Futures does not support RPC-style WebSocket trading like Spot.
//! All trading operations are done via REST API. WebSocket is only used for streaming.

pub mod client;
pub mod sign;
pub mod streaming_client;
pub mod types;
pub mod ws_client;

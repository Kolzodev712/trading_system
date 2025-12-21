# Trading System TODO


### Exchange engine
- Implement Rate limit logic:
    - Add generic types for different kinds of RL algorithms.
    - Add binance specific RL implementation => https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/rate-limits

### Make Strategies Work and Post Orders
- [ ] Ensure strategy event loop processes orders correctly
- [ ] Fix any issues with strategy order placement
- [ ] Verify strategy cancellation works properly
- [ ] Test strategy lifecycle (enable/disable, order management)
- [ ] Ensure strategies can trade on multiple pairs independently



### Documentation
- [ ] Document level configuration format & level usage
- [ ] Document strategy configuration format & strategies usage
- [ ] Update architecture docs with current implementation
- [ ] Add examples for level and strategy configurations


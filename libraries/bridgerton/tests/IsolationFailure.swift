// Must fail compilation: a normal synchronous function cannot access a bridged object.
func illegalAccess(_ counter: Counter) -> UInt32 {
    counter.value()
}

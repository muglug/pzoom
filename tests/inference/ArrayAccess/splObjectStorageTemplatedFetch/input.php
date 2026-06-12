<?php
final class N {}
final class U4 {}

final class P {
    /** @var SplObjectStorage<N, U4> */
    private SplObjectStorage $node_types;

    public function __construct() { $this->node_types = new SplObjectStorage(); }

    public function getType(N $node): ?U4 {
        return $this->node_types[$node] ?? null;
    }
}

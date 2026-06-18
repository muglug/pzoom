<?php
/** @psalm-immutable */
final class Bag {
    /** @var array<string,int> */
    public array $items = [];

    public function sortedClone(): self {
        $cloned = clone $this;
        ksort($cloned->items);
        return $cloned;
    }
}

/** @psalm-immutable */
class Other {
    /** @var array<string,int> */
    public array $items = [];
}

final class Wrapper {
    public function leaveAlone(Other $o): void {
        array_shift($o->items);
    }
}

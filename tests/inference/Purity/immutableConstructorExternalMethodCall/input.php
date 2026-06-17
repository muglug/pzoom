<?php

final class Source {
    public function read(): int {
        return 1;
    }
}

/** @psalm-immutable */
final class Snapshot {
    public int $value;
    public int $extra;

    public function __construct(Source $source, Source $other) {
        // External, non-mutation-free calls are impure here.
        $this->value = $source->read();
        $this->extra = $other->read();
        // Mutating $this (assignment above, self call below) is allowed in the
        // external-mutation-free constructor context.
        $this->normalize();
    }

    private function normalize(): void {}
}

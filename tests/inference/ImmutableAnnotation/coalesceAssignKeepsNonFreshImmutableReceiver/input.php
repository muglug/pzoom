<?php
/** @psalm-immutable */
final class Box {
    public function __construct(public string $v) {}

    public function carried(?Box $seed, string $a): self {
        $cloned = $seed;
        $cloned ??= clone $this;
        $cloned->v = $a;
        return $cloned;
    }

    public function fresh(string $a): self {
        $cloned = clone $this;
        $cloned->v = $a;
        return $cloned;
    }
}

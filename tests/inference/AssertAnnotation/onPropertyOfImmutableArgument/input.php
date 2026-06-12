<?php
/** @psalm-immutable */
class Aclass {
    public ?string $b;
    public function __construct(?string $b) {
        $this->b = $b;
    }
}

/** @psalm-assert !null $item->b */
function c(\Aclass $item): void {
    if (null === $item->b) {
        throw new \InvalidArgumentException("");
    }
}

/** @var \Aclass $a */
c($a);
echo strlen($a->b);

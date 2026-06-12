<?php
class B {
    public ?string $c;
    public function __construct(?string $c) {
        $this->c = $c;
    }
}

/** @psalm-immutable */
class Aclass {
    public B $b;
    public function __construct(B $b) {
        $this->b = $b;
    }
}

/** @psalm-assert-if-false !null $item->b->c */
function c(\Aclass $item): bool {
    return null !== $item->b->c;
}

$a = new \Aclass(new \B(null));
if (!c($a)) {
    echo strlen($a->b->c);
}

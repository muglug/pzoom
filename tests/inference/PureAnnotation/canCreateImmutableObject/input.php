<?php
/** @psalm-immutable */
class A {
    private string $s;

    public function __construct(string $s) {
        $this->s = $s;
    }

    public function getShort() : string {
        return substr($this->s, 0, 5);
    }
}

/** @psalm-pure */
function makeA(string $s) : A {
    $a = new A($s);

    if ($a->getShort() === "bar") {
        return new A("foo");
    }

    return $a;
}

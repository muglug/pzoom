<?php
final class A {
    private string $s;

    public function __construct(string $s) {
        $this->s = $s;
    }

    /** @psalm-mutation-free */
    public function getShort() : string {
        return substr($this->s, 0, 5);
    }
}

$a = new A("hello");
$a->getShort();

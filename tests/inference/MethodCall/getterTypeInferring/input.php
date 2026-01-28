<?php
class A {
    /** @var int|string|null */
    public $a;

    /** @return int|string|null */
    final public function getValue() {
        return $this->a;
    }
}

$a = new A();

if (is_string($a->getValue())) {
    echo strlen($a->getValue());
}

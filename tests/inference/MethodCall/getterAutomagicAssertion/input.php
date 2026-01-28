<?php
class A {
    /** @var string|null */
    public $a;

    /** @return string|null */
    final public function getA() {
        return $this->a;
    }
}

$a = new A();

if ($a->getA() !== null) {
    echo strlen($a->getA());
}

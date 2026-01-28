<?php
class A {
    /** @return A|false */
    public function getParent() {
        return rand(0, 1) ? new A : false;
    }
}

$a = new A();

do {
    $a = $a->getParent();
} while ($a !== false);

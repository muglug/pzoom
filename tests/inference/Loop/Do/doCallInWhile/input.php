<?php
class A {
    public function getParent() : ?A {
        return rand(0, 1) ? new A : null;
    }
}

$a = new A();
$i = 0;
do {
    $i++;
} while ($a = $a->getParent());

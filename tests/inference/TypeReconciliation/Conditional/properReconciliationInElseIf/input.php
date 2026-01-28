<?php
class A {}
$a = rand(0, 1) ? new A : null;

if (rand(0, 1)) {
    $a = new A();
} elseif (!$a) {
    $a = new A();
}

if ($a) {}

<?php
class B {
    const C = A;
}

const A = 5;

$a = B::C;

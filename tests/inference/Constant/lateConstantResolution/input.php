<?php
class A {
    const FOO = "foo";
}

class B {
    const BAR = [
        A::FOO
    ];
    const BAR2 = A::FOO;
}

$a = B::BAR[0];
$b = B::BAR2;

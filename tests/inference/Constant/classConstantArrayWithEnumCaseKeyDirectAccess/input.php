<?php
enum E {
    case K1;
    case K2;
}
enum BEI: int {
    case K3 = 1;
    case K4 = 2;
}
enum BES: string {
    case K5 = "a";
    case K6 = "b";
}
class A {
    public const C = [
        E::K1->name => "c",
        E::K2->name => 3,
        BEI::K3->name => "d",
        BEI::K4->name => 4,
        BEI::K3->value => "e",
        BEI::K4->value => 5,
        BES::K5->name => "f",
        BES::K6->name => 6,
        BES::K5->value => "g",
        BES::K6->value => 7,
    ];
}
$a = A::C[E::K1->name];
$b = A::C[E::K2->name];
$c = A::C[BEI::K3->name];
$d = A::C[BEI::K4->name];
$e = A::C[BEI::K3->value];
$f = A::C[BEI::K4->value];
$g = A::C[BES::K5->name];
$h = A::C[BES::K6->name];
$i = A::C[BES::K5->value];
$j = A::C[BES::K6->value];
$k = A::C["K1"];
$l = A::C["K2"];
$m = A::C["K3"];
$n = A::C["K4"];
$o = A::C[1];
$p = A::C[2];
$q = A::C["K5"];
$r = A::C["K6"];
$s = A::C["a"];
$t = A::C["b"];

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
const C = [
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
$a = C[E::K1->name];
$b = C[E::K2->name];
$c = C[BEI::K3->name];
$d = C[BEI::K4->name];
$e = C[BEI::K3->value];
$f = C[BEI::K4->value];
$g = C[BES::K5->name];
$h = C[BES::K6->name];
$i = C[BES::K5->value];
$j = C[BES::K6->value];
$k = C["K1"];
$l = C["K2"];
$m = C["K3"];
$n = C["K4"];
$o = C[1];
$p = C[2];
$q = C["K5"];
$r = C["K6"];
$s = C["a"];
$t = C["b"];

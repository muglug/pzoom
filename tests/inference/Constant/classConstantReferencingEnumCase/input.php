<?php
enum E {
    case Z;
}
class C {
    public const CC = E::Z;
}
$c = C::CC;

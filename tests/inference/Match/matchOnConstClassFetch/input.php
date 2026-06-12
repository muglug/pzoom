<?php
final class Obj1 {
    public string $propFromObj1 = "str";
}
final class Obj2 {
    public int $propFromObj2 = 42;
}

function process(Obj1|Obj2 $obj): int|string
{
    return match ($obj::class) {
        Obj1::class => $obj->propFromObj1,
        Obj2::class => $obj->propFromObj2,
    };
}

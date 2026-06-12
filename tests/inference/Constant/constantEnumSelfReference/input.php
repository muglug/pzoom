<?php
enum Bar: string {
    case A = "a";
    case B = "b";
    public const STR = self::A->value . self::B->value;
}

class Foo {
    public const CONCAT_STR = "a" . Bar::STR . "e";
}

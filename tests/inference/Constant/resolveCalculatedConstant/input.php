<?php
interface Types {
    public const TWO = "two";
}

interface A {
   public const TYPE_ONE = "one";
   public const TYPE_TWO = Types::TWO;
}

class B implements A {
    public function __construct()
    {
        echo self::TYPE_ONE;
        echo self::TYPE_TWO;
    }
}

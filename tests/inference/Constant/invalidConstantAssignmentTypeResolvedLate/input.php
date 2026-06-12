<?php
class Foo
{
    /** @var int */
    public const BAR = "bar" . self::BAZ;
    public const BAZ = "baz";
    public const BARBAZ = self::BAR . self::BAZ;
}

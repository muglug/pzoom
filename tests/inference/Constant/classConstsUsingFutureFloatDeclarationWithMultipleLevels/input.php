<?php
class Foo {
    public const BAZ = self::BAR + 1.0;
    public const BAR = self::FOO + 1.0;
    public const FOO = 1.0;
}

<?php
class Foo
{
    public static function someInt(): int
    {
        return 1;
    }
}

/**
 * @return int
 */
function makeInt()
{
    $fooClass = Foo::class;
    return $fooClass::someInt();
}

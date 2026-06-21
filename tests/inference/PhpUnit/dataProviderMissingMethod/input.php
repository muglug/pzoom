<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class FooTest extends TestCase
{
    /**
     * @dataProvider provideMissing
     */
    public function testThing(int $x): void
    {
        echo $x;
    }
}

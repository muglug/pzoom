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
     * @dataProvider provideData
     */
    public function testThing(int $x): void
    {
        echo $x;
    }

    /**
     * @return iterable<array-key, array{string}>
     */
    public function provideData(): iterable
    {
        return [['hello']];
    }
}

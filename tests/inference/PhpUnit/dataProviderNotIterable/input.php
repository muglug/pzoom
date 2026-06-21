<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class BarTest extends TestCase
{
    /**
     * @dataProvider provideData
     */
    public function testThing(int $x): void
    {
        echo $x;
    }

    public function provideData(): int
    {
        return 5;
    }
}

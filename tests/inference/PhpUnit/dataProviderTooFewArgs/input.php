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
    public function testThing(int $x, string $y): void
    {
        echo $x . $y;
    }

    /**
     * @return iterable<array-key, array{int}>
     */
    public function provideData(): iterable
    {
        return [[1]];
    }
}

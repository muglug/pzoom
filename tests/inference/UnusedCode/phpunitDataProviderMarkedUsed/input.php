<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class BazTest extends TestCase
{
    /**
     * @dataProvider provideData
     */
    public function testThing(int $x): void
    {
        echo $x;
    }

    /**
     * @return iterable<array-key, array{int}>
     */
    public function provideData(): iterable
    {
        return [[1], [2]];
    }
}

<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class SharedProviders
{
    /**
     * @return iterable<array-key, array{int}>
     */
    public function provide(): iterable
    {
        return [[1], [2]];
    }
}

final class QuxTest extends TestCase
{
    /**
     * @dataProvider \App\Tests\SharedProviders::provide
     */
    public function testThing(int $x): void
    {
        echo $x;
    }
}

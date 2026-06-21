<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ExampleTest extends TestCase
{
    public function testSomething(): void
    {
    }
}

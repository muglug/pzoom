<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class FeatureTest extends TestCase
{
    /**
     * @test
     */
    public function itWorks(): void
    {
    }
}

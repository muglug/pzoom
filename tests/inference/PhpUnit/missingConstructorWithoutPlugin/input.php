<?php

namespace PHPUnit\Framework;

abstract class TestCase
{
}

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ServiceTest extends TestCase
{
    private string $service;

    protected function setUp(): void
    {
        $this->service = 'service';
    }

    public function testService(): void
    {
        echo $this->service;
    }
}

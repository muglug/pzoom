<?php

namespace PHPUnit\Framework {
    abstract class TestCase
    {
    }
}

namespace PHPUnit\Framework\Attributes {
    #[\Attribute]
    final class DataProvider
    {
        public function __construct(string $methodName)
        {
        }
    }
}

namespace App\Tests {
    use PHPUnit\Framework\TestCase;
    use PHPUnit\Framework\Attributes\DataProvider;

    final class BarTest extends TestCase
    {
        #[DataProvider('provideData')]
        public function testThing(int $x): void
        {
            echo $x;
        }

        public function provideData(): int
        {
            return 5;
        }
    }
}

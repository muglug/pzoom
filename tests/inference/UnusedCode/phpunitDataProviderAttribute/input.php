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
        public function __construct(public readonly string $methodName)
        {
        }
    }
}

namespace App\Tests {
    use PHPUnit\Framework\TestCase;
    use PHPUnit\Framework\Attributes\DataProvider;

    final class WidgetTest extends TestCase
    {
        #[DataProvider('provideData')]
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
}

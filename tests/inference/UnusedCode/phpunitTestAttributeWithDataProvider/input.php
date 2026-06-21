<?php

namespace PHPUnit\Framework {
    abstract class TestCase
    {
    }
}

namespace PHPUnit\Framework\Attributes {
    #[\Attribute]
    final class Test
    {
    }

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
    use PHPUnit\Framework\Attributes\Test;
    use PHPUnit\Framework\Attributes\DataProvider;

    final class GizmoTest extends TestCase
    {
        #[Test]
        #[DataProvider('provideData')]
        public function itAddsUp(int $x): void
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

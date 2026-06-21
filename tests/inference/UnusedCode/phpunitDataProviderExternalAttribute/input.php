<?php

namespace PHPUnit\Framework {
    abstract class TestCase
    {
    }
}

namespace PHPUnit\Framework\Attributes {
    #[\Attribute]
    final class DataProviderExternal
    {
        public function __construct(
            public readonly string $className,
            public readonly string $methodName,
        ) {
        }
    }
}

namespace App\Tests {
    use PHPUnit\Framework\TestCase;
    use PHPUnit\Framework\Attributes\DataProviderExternal;

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

    final class GadgetTest extends TestCase
    {
        #[DataProviderExternal(SharedProviders::class, 'provide')]
        public function testThing(int $x): void
        {
            echo $x;
        }
    }
}

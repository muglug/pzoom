<?php

namespace PHPUnit\Framework {
    abstract class TestCase
    {
    }
}

namespace PHPUnit\Framework\Attributes {
    /**
     * @api
     */
    #[\Attribute]
    final class Test
    {
    }
}

namespace App\Tests {
    use PHPUnit\Framework\TestCase;
    use PHPUnit\Framework\Attributes\Test;

    final class FeatureTest extends TestCase
    {
        #[Test]
        public function itWorks(): void
        {
        }
    }
}

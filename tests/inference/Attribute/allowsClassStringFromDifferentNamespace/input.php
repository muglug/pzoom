<?php

namespace NamespaceOne {
    use Attribute;

    #[Attribute(Attribute::TARGET_CLASS)]
    class FooAttribute
    {
        /** @var class-string */
        private string $className;

        /**
         * @param class-string<FoobarInterface> $className
         */
        public function __construct(string $className)
        {
            $this->className = $className;
        }
    }

    interface FoobarInterface {}

    class Bar implements FoobarInterface {}
}

namespace NamespaceTwo {
    use NamespaceOne\FooAttribute;
    use NamespaceOne\Bar as ZZ;

    #[FooAttribute(className: ZZ::class)]
    class Baz {}
}
                
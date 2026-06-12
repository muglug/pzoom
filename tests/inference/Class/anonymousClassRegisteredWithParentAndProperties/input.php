<?php

namespace Foo\Bar {
    abstract class Constraint
    {
        abstract public function toString(): string;

        protected function exporter(): string
        {
            return 'x';
        }
    }
}

namespace Psalm\Tests {
    use Foo\Bar\Constraint;

    class T
    {
        public function conciseExpected(Constraint $inner): Constraint
        {
            return new class ($inner) extends Constraint
            {
                private Constraint $inner;

                public function __construct(Constraint $inner)
                {
                    $this->inner = $inner;
                }

                public function toString(): string
                {
                    return $this->inner->toString();
                }

                protected function failureDescription(): string
                {
                    return $this->exporter() . ' ' . $this->toString();
                }
            };
        }
    }
}

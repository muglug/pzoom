<?php
/** @template-covariant T */
class Foo
{
    /** @param \Closure(string):T $closure */
    public function __construct($closure) {}
}

class Bar
{
    /** @var Foo<array> */
    private $FooArray;

    public function __construct() {
        $this->FooArray = new Foo(function(string $s): array {
            /** @psalm-suppress MixedAssignment */
            $json = \json_decode($s, true);

            if (! \is_array($json)) {
                return [];
            }

            return $json;
        });

        takesFooArray($this->FooArray);
    }
}

/** @param Foo<array> $_ */
function takesFooArray($_): void {}
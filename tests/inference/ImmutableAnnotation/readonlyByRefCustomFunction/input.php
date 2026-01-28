<?php
namespace World;

final class Foo
{
    /**
     * @readonly
     */
    public array $values;

    public function __construct(array $values)
    {
        $this->values = $values;
    }
}

/**
 * @param string $a
 * @param array $b
 * @return void
 */
function bar($a, &$b) {}

$x = new Foo([]);
bar("hello", $x->values);

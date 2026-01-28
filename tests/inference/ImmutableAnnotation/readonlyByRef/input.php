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

$x = new Foo([]);
reset($x->values);

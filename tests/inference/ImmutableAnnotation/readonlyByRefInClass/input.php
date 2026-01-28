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

    /**
     * @return mixed
     */
    public function bar()
    {
        return reset($this->values);
    }
}

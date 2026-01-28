<?php
class Foo {}
class Bar {}

/** @template FooOrBarOrNull of Foo|Bar|null */
class Resolved
{
    /**
     * @var FooOrBarOrNull
     */
    private $entity = null;

    /**
     * @psalm-param FooOrBarOrNull $qux
     */
    public function __construct(?object $qux)
    {
        if ($qux instanceof Foo) {
            $this->entity = $qux;
        }
    }
}

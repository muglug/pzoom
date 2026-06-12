<?php
class Foo {
    /** @var Foo|null */
    protected $bar;

    /**
     * @return ?Foo
     */
    function getBarWithIsset() {
        if (isset($this->bar)) return $this->bar;
        return null;
    }
}

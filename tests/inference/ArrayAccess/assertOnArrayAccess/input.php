<?php
class A {
    public function foo() : void {}
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class C implements ArrayAccess
{
    /**
     * @var array
     */
    protected $data = [];

    /**
     * @param string $name
     * @return mixed
     */
    public function offsetGet($name)
    {
        return $this->data[$name];
    }

    /**
     * @param string $offset
     * @param mixed $value
     */
    public function offsetSet($offset, $value) : void
    {
        $this->data[$offset] = $value;
    }

    public function __isset(string $name) : bool
    {
        return isset($this->data[$name]);
    }

    public function __unset(string $name) : void
    {
        unset($this->data[$name]);
    }

    /**
     * @psalm-suppress MixedArgument
     */
    public function offsetExists($offset) : bool
    {
        return $this->__isset($offset);
    }

    /**
     * @psalm-suppress MixedArgument
     */
    public function offsetUnset($offset) : void
    {
        $this->__unset($offset);
    }
}

$container = new C();
if ($container["a"] instanceof A) {
    $container["a"]->foo();
}

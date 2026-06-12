<?php
$array = new C([]);
$array["key"] = [];
/** @psalm-suppress PossiblyInvalidArrayAssignment */
$array["key"][] = "testing";

$c = isset($array["foo"]) ? $array["foo"] : null;

/**
 * @psalm-suppress MissingTemplateParam
 */
class C implements ArrayAccess
{
    /**
     * @var array<C|scalar>
     */
    protected $data = [];

    /**
     * @param array<scalar|array> $array
     * @psalm-suppress MixedArgumentTypeCoercion
     */
    final public function __construct(array $array)
    {
        foreach ($array as $key => $value) {
            if (is_array($value)) {
                $this->data[$key] = new static($value);
            } else {
                $this->data[$key] = $value;
            }
        }
    }

    /**
     * @param string $name
     * @return C|scalar
     */
    public function offsetGet($name)
    {
        return $this->data[$name];
    }

    /**
     * @param ?string $offset
     * @param scalar|array $value
     * @psalm-suppress MixedArgumentTypeCoercion
     */
    public function offsetSet($offset, $value) : void
    {
        if (is_array($value)) {
            $value = new static($value);
        }

        if (null === $offset) {
            $this->data[] = $value;
        } else {
            $this->data[$offset] = $value;
        }
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

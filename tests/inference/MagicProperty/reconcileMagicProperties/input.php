<?php
/**
 * @property string|null $a A
 * @property string|null $b B
 */
class Foo
{
    private array $props = [];

    public function __construct() {
        $this->props["a"] = "hello";
        $this->props["b"] = "goodbye";
    }

    /**
     * @psalm-mutation-free
     */
    public function __get(string $prop){
        return $this->props[$prop] ?? null;
    }

    /** @param mixed $b */
    public function __set(string $a, $b){
        $this->props[$a] = $b;
    }

    public function bar(): string {
        if (is_null($this->a) || is_null($this->b)) {

        } else {
            return $this->b;
        }

        return "hello";
    }
}

<?php

class A {
    /** @var array<string, string> */
    private array $itemAttr = [];

    /**
     * @template T as ?string
     * @param T $name
     * @return string|string[]
     * @psalm-return (T is string ? string : array<string, string>)
     */
    public function getAttribute(?string $name, string $default = "")
    {
        if (null === $name) {
            return $this->itemAttr;
        }
        return isset($this->itemAttr[$name]) ? $this->itemAttr[$name] : $default;
    }
}

$a = (new A)->getAttribute("colour", "red"); // typed as string
$b = (new A)->getAttribute(null); // typed as array<string, string>
$c = (new A)->getAttribute($GLOBALS["foo"]); // typed as string|array<string, string>

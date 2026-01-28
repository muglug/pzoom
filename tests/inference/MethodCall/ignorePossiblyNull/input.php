<?php
class Foo {
    protected ?string $type = null;

    public function prepend(array $arr) : string {
        return $this->getType();
    }

    /**
     * @psalm-ignore-nullable-return
     */
    public function getType() : ?string
    {
        return $this->type;
    }
}

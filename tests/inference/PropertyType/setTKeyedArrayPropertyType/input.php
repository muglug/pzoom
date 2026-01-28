<?php
class Foo {
    /**
     * @psalm-var array{from:bool, to:bool}
     */
    protected $changed = [
        "from" => false,
        "to" => false,
    ];

    /**
     * @psalm-param "from"|"to" $property
     */
    public function ChangeThing(string $property) : void {
        $this->changed[$property] = true;
    }
}

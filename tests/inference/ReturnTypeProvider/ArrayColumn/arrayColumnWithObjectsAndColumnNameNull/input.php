<?php
class C {
    /** @var string */
    public $name = "";
    public function foo(): void {}
}

foreach (array_column([new C, new C], null, "name") as $instance) {
    $instance->foo();
}
            

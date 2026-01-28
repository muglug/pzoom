<?php
class SomeClass {}

interface SomeInterface {
    public function doStuff(): void;
}

function takesAorB(SomeClass|SomeInterface $some): void {
    if ($some instanceof SomeInterface) {
        $some->doStuff();
    }
}

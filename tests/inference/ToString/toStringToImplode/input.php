<?php
class Bar {
    public function __toString() {
        return "foo";
    }
}

echo implode(":", [new Bar()]);

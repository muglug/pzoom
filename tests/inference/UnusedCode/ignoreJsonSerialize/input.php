<?php
final class Foo implements JsonSerializable {
    public function jsonSerialize() : array {
        return [];
    }
}

new Foo();

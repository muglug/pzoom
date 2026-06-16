<?php
final class Foo implements Serializable {
    public function serialize() : string {
        return "";
    }

    public function unserialize(string $data) : void {}
}

new Foo();

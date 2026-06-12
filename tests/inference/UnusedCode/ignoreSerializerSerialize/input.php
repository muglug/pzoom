<?php
final class Foo implements Serializable {
    public function serialize() : string {
        return "";
    }

    public function unserialize($data) : void {}
}

new Foo();

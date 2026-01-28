<?php
class Foo implements \Serializable {
    /** @param int $data */
    public function unserialize($data) {}
    public function serialize() {}
}

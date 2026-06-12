<?php
class A implements Serializable {
    /** @var int */
    private $id = 1;

    /**
     * @param string $data
     */
    public function unserialize($data) : void
    {
        [
            $this->id,
        ] = (array) \unserialize($data);
    }

    public function serialize() : string
    {
        return serialize([$this->id]);
    }
}

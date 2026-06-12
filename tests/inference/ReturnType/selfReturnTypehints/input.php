<?php
class A {
    /**
     * @return static
     */
    public function getMe(): self
    {
        return $this;
    }
}

class B extends A
{
    /**
     * @return static
     */
    public function getMeAgain(): self {
        return $this->getMe();
    }
}

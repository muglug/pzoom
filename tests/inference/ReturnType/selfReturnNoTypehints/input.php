<?php
class A {
    /**
     * @return static
     */
    public function getMe()
    {
        return $this;
    }
}

class B extends A
{
    /**
     * @return static
     */
    public function getMeAgain() {
        return $this->getMe();
    }
}

<?php

class Bar
{
    /**
     * @deprecated
     */
    public static bool $deprecatedProperty = false;
}

Bar::$deprecatedProperty;

<?php
/** @return 'getMethInFunc' */
function getMethInFunc(): string{
    return __METHOD__;
}

/** @return 'getFuncInFunc' */
function getFuncInFunc(): string{
    return __FUNCTION__;
}

class A{
    /** @return 'A::getMethInMeth' */
    function getMethInMeth(): string{
        return __METHOD__;
    }

    /** @return 'getFuncInMeth' */
    function getFuncInMeth(): string{
        return __FUNCTION__;
    }
}

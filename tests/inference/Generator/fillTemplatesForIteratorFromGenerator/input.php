<?php
/**
 * @return Generator<int, string>
 */
function generator(): Generator
{
    yield "test";
}

$iterator = new NoRewindIterator(generator());
                    

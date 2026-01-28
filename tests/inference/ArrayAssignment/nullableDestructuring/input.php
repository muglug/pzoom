<?php
/**
 * @return array{"foo", "bar"}|null
 */
function foobar(): ?array
{
    return null;
}

[$_foo, $_bar] = foobar();
                    

<?php
/**
 * @return 7
 */
function scope(){
    return 1 | 2 | 4 | (1 & 0);
}

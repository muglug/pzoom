<?php
/** @psalm-suppress StringIncrement */
for($a = "a"; $a != "z"; $a++){
    if($a === "b"){
        echo "b reached";
    }
}

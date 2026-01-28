<?php
                    namespace A\B {
                        interface Bar {};

                        /**
                         * @psalm-internal A\B
                         */
                        class Foo { }
                    }

                    namespace A\B\C {
                        class Bat {
                            public function batBat(\A\B\Bar $bar) : void {
                                $bar instanceOf \A\B\Foo;
                            }
                        }
                    }

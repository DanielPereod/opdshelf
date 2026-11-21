package io.github.danielpereod.opdshelf.data

/**
 * How far this device's clock is from the server's, learnt from the `server_time` of its answers.
 * Times of highlights are kept in the server's clock, as the server's answers give them, so a phone
 * whose clock is off still compares its changes with the others' correctly; they are sent in the
 * device's own clock together with `sent_at`, which is how the server corrects them.
 */
object ServerClock {
    /** Seconds to add to this device's clock to get the server's. */
    @Volatile
    var offset: Long = 0
        private set

    fun learn(serverTime: Long, deviceNow: Long = System.currentTimeMillis() / 1000) {
        if (serverTime > 0) offset = serverTime - deviceNow
    }

    fun now(deviceNow: Long = System.currentTimeMillis() / 1000): Long = deviceNow + offset

    /** What the server has to be sent for a time of its own clock. */
    fun toDevice(serverTime: Long): Long = serverTime - offset

    fun forget() {
        offset = 0
    }
}

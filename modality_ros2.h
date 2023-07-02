#ifndef MODALITY_ROS2_H
#define MODALITY_ROS2_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

#ifndef MODALITY_ROS2_WEAK_ATTRIBUTE
#define MODALITY_ROS2_WEAK_ATTRIBUTE __attribute__((weak))
#endif

/* Set the external nonce to be recorded on the publisher's next publish event */
void modality_ros2_set_next_publisher_nonce(void *rmw_publisher, uint64_t nonce);
/* Clear the external nonce for a given publisher */
void modality_ros2_clear_next_publisher_nonce(void *rmw_publisher);

/* Set the external nonce to be recorded on the subscription's next subscribe event */
void modality_ros2_set_next_subscription_nonce(void *rmw_subscription, uint64_t nonce);
/* Get the external nonce that was previously recorded on a subscribe event, the nonce is post-incremented */
void modality_ros2_get_next_subscription_nonce(void *rmw_subscription, uint64_t *nonce);
/* Clear the external nonce for a given subscription */
void modality_ros2_clear_next_subscription_nonce(void *rmw_subscription);

/* This macro provides a default definition for the external nonce API.
 * NOTE: They don't get called when using the LD_PRELOAD integration. */
#define MODALITY_ROS2_DEFAULT_NONCE_IMPLS \
    void MODALITY_ROS2_WEAK_ATTRIBUTE modality_ros2_set_next_publisher_nonce(void *rmw_publisher, uint64_t nonce) {} \
    void MODALITY_ROS2_WEAK_ATTRIBUTE modality_ros2_clear_next_publisher_nonce(void *rmw_publisher) {} \
    void MODALITY_ROS2_WEAK_ATTRIBUTE modality_ros2_set_next_subscription_nonce(void *rmw_subscription, uint64_t nonce) {} \
    void MODALITY_ROS2_WEAK_ATTRIBUTE modality_ros2_get_next_subscription_nonce(void *rmw_subscription, uint64_t *nonce) {} \
    void MODALITY_ROS2_WEAK_ATTRIBUTE modality_ros2_clear_next_subscription_nonce(void *rmw_subscription) {}

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MODALITY_ROS2_H */

#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#include "rmw/init.h"
#include "rmw/types.h"
#endif

#include <dlfcn.h>
#include <stdio.h>
#include <time.h>

#include "rmw/rmw.h"

/***************************************************************************/
/* Hook functions that are called after the underlying rmw invocation.The  */
/* passthrough invocations happen in C so we don't have Rust frames on the */
/* stack at the same time; sometimes the rmw layer throws C++ exceptions,  */
/* and that's bad mojo (undefined behavior) in Rust.                       */
/***************************************************************************/

void modality_after_rmw_create_node(const char *name, const char *namespace,
                                    rmw_node_t *node_ptr);

void modality_after_rmw_destroy_node(const uintptr_t destroyed_node_addr);

void modality_after_rmw_create_publisher(
    const rmw_node_t *node, const rmw_publisher_t *publisher,
    const rosidl_message_type_support_t *type_support, const char *topic_name);

void modality_after_rmw_destroy_publisher(
    const uintptr_t destroyed_publisher_addr);

void modality_before_rmw_publish(const rmw_publisher_t *publisher_ptr,
                                 const void *message);

void modality_after_clock_gettime(clockid_t clockid, struct timespec *tp);

void modality_after_rmw_create_subscription(
    const rmw_node_t *node, const rmw_subscription_t *subscription,
    const rosidl_message_type_support_t *type_support, const char *topic_name);

void modality_after_rmw_destroy_subscription(
    const uintptr_t destroyed_subscription_addr);

void modality_after_rmw_take_with_info(const rmw_subscription_t *sub_ptr,
                                       void *ros_message,
                                       rmw_message_info_t *message_info);

void modality_after_rmw_create_service(
    const rmw_node_t *node,
    const rmw_service_t *service,
    const rosidl_service_type_support_t *type_support,
    const char *service_name);

void modality_after_rmw_destroy_service(const uintptr_t destroyed_service_addr);

void modality_after_rmw_create_client(
    const rmw_node_t *node,
    const rmw_client_t *client,
    const rosidl_service_type_support_t *type_support,
    const char *service_name);

void modality_after_rmw_destroy_client(const uintptr_t destroyed_client_addr);

void modality_after_rmw_send_request(const rmw_client_t *client,
                                     const void *ros_request,
                                     int64_t sequence_id);

void modality_after_rmw_take_request(const rmw_service_t *service,
                                     rmw_service_info_t *request_header,
                                     void *ros_request);

void modality_after_rmw_send_response(const rmw_service_t *service,
                                      rmw_request_id_t *request_header,
                                      void *ros_response);

void modality_after_rmw_take_response(const rmw_client_t *client,
                                      rmw_service_info_t *request_header,
                                      void *ros_response);

/***************************************/
/* Prototypes for the hooked functions */
/***************************************/

typedef rmw_node_t *(*rmw_create_node_t)(rmw_context_t *rmw_context_t,
                                         const char *name,
                                         const char *namespace_);

typedef rmw_ret_t (*rmw_destroy_node_t)(rmw_node_t *node);

typedef rmw_publisher_t *(*rmw_create_publisher_t)(
    const rmw_node_t *node, const rosidl_message_type_support_t *type_support,
    const char *topic_name, const rmw_qos_profile_t *qos_profile,
    const rmw_publisher_options_t *publisher_options);

typedef rmw_ret_t (*rmw_destroy_publisher_t)(rmw_node_t *node,
                                             rmw_publisher_t *publisher_ptr);

typedef rmw_ret_t (*rmw_publish_t)(const rmw_publisher_t *pub_ptr,
                                   const void *message,
                                   rmw_publisher_allocation_t *allocation);

typedef rmw_ret_t (*clock_gettime_t)(clockid_t clockid, struct timespec *tp);

typedef rmw_subscription_t *(*rmw_create_subscription_t)(
    const rmw_node_t *node_ptr,
    const rosidl_message_type_support_t *type_support, const char *topic_name,
    const rmw_qos_profile_t *qos_policies,
    const rmw_subscription_options_t *subscription_options);

typedef rmw_ret_t (*rmw_destroy_subscription_t)(const rmw_node_t *node_ptr,
                                                rmw_subscription_t *sub_ptr);

typedef rmw_ret_t (*rmw_take_with_info_t)(
    const rmw_subscription_t *subscription, void *ros_message, bool *taken,
    rmw_message_info_t *message_info,
    rmw_subscription_allocation_t *allocation);

typedef rmw_service_t *(*rmw_create_service_t)(
    const rmw_node_t *node, const rosidl_service_type_support_t *type_support,
    const char *service_name, const rmw_qos_profile_t *qos_profile);

typedef rmw_ret_t (*rmw_destroy_service_t)(rmw_node_t *node,
                                           rmw_service_t *service);

typedef rmw_client_t *(*rmw_create_client_t)(
    const rmw_node_t *node, const rosidl_service_type_support_t *type_support,
    const char *service_name, const rmw_qos_profile_t *qos_policies);

typedef rmw_ret_t (*rmw_destroy_client_t)(rmw_node_t *node,
                                          rmw_client_t *client);

typedef rmw_ret_t (*rmw_send_request_t)(const rmw_client_t *client,
                                        const void *ros_request,
                                        int64_t *sequence_id);

typedef rmw_ret_t (*rmw_take_request_t)(const rmw_service_t *service,
                                        rmw_service_info_t *request_header,
                                        void *ros_request, bool *taken);

typedef rmw_ret_t (*rmw_send_response_t)(const rmw_service_t *service,
                                         rmw_request_id_t *request_header,
                                         void *ros_response);

typedef rmw_ret_t (*rmw_take_response_t)(const rmw_client_t *client,
                                         rmw_service_info_t *request_header,
                                         void *ros_response, bool *taken);

/************************************************************/
/* Pointers to underlying versions of the hooked functions, */
/* initialized on demand.                                   */
/************************************************************/

static rmw_create_node_t real_rmw_create_node;
static rmw_destroy_node_t real_rmw_destroy_node;
static rmw_create_publisher_t real_rmw_create_publisher;
static rmw_destroy_publisher_t real_rmw_destroy_publisher;
static rmw_publish_t real_rmw_publish;
static clock_gettime_t real_clock_gettime;
static rmw_create_subscription_t real_rmw_create_subscription;
static rmw_destroy_subscription_t real_rmw_destroy_subscription;
static rmw_take_with_info_t real_rmw_take_with_info;
static rmw_create_service_t real_rmw_create_service;
static rmw_destroy_service_t real_rmw_destroy_service;
static rmw_create_client_t real_rmw_create_client;
static rmw_destroy_client_t real_rmw_destroy_client;
static rmw_send_request_t real_rmw_send_request;
static rmw_take_request_t real_rmw_take_request;
static rmw_send_response_t real_rmw_send_response;
static rmw_take_response_t real_rmw_take_response;

/***********************/
/* Hook function impls */
/***********************/

rmw_node_t *rmw_create_node(rmw_context_t *context, const char *name,
                            const char *namespace) {
  if (!real_rmw_create_node) {
    real_rmw_create_node = dlsym(RTLD_NEXT, "rmw_create_node");
  }

  rmw_node_t *node = real_rmw_create_node(context, name, namespace);
  if (node != NULL) {
    modality_after_rmw_create_node(name, namespace, node);
  }

  return node;
}

rmw_ret_t rmw_destroy_node(rmw_node_t *node_ptr) {
  if (!real_rmw_destroy_node) {
    real_rmw_destroy_node = dlsym(RTLD_NEXT, "rmw_destroy_node");
  }

  rmw_ret_t ret = real_rmw_destroy_node(node_ptr);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_destroy_node((uintptr_t)node_ptr);
  }

  return ret;
}

rmw_publisher_t *rmw_create_publisher(
    const rmw_node_t *node, const rosidl_message_type_support_t *type_support,
    const char *topic_name, const rmw_qos_profile_t *qos_profile,
    const rmw_publisher_options_t *publisher_options) {
  if (!real_rmw_create_publisher) {
    real_rmw_create_publisher = dlsym(RTLD_NEXT, "rmw_create_publisher");
  }

  rmw_publisher_t *publisher = real_rmw_create_publisher(
      node, type_support, topic_name, qos_profile, publisher_options);
  if (publisher != NULL) {
    modality_after_rmw_create_publisher(node, publisher, type_support,
                                        topic_name);
  }

  return publisher;
}

rmw_ret_t rmw_destroy_publisher(rmw_node_t *node_ptr,
                                rmw_publisher_t *pub_ptr) {
  if (!real_rmw_destroy_publisher) {
    real_rmw_destroy_publisher = dlsym(RTLD_NEXT, "rmw_destroy_publisher");
  }

  rmw_ret_t ret = real_rmw_destroy_publisher(node_ptr, pub_ptr);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_destroy_publisher((uintptr_t)pub_ptr);
  }

  return ret;
}

rmw_ret_t rmw_publish(const rmw_publisher_t *publisher, const void *ros_message,
                      rmw_publisher_allocation_t *allocation) {
  if (!real_rmw_publish) {
    real_rmw_publish = dlsym(RTLD_NEXT, "rmw_publish");
  }

  modality_before_rmw_publish(publisher, ros_message);
  return real_rmw_publish(publisher, ros_message, allocation);
}

int clock_gettime(clockid_t clockid, struct timespec *tp) {
  if (!real_clock_gettime) {
    real_clock_gettime = dlsym(RTLD_NEXT, "clock_gettime");
  }

  int ret = real_clock_gettime(clockid, tp);
  // 0 means success
  if (ret == 0) {
    modality_after_clock_gettime(clockid, tp);
  }

  return ret;
}

rmw_subscription_t *rmw_create_subscription(
    const rmw_node_t *node, const rosidl_message_type_support_t *type_support,
    const char *topic_name, const rmw_qos_profile_t *qos_policies,
    const rmw_subscription_options_t *subscription_options) {
  if (!real_rmw_create_subscription) {
    real_rmw_create_subscription = dlsym(RTLD_NEXT, "rmw_create_subscription");
  }

  rmw_subscription_t *subscription = real_rmw_create_subscription(
      node, type_support, topic_name, qos_policies, subscription_options);
  if (subscription != NULL) {
    modality_after_rmw_create_subscription(node, subscription, type_support,
                                           topic_name);
  }

  return subscription;
}

rmw_ret_t rmw_destroy_subscription(rmw_node_t *node,
                                   rmw_subscription_t *subscription) {
  if (!real_rmw_destroy_subscription) {
    real_rmw_destroy_subscription =
        dlsym(RTLD_NEXT, "rmw_destroy_subscription");
  }

  rmw_ret_t ret = real_rmw_destroy_subscription(node, subscription);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_destroy_subscription((uintptr_t)subscription);
  }

  return ret;
}

rmw_ret_t rmw_take_with_info(const rmw_subscription_t *subscription,
                             void *ros_message, bool *taken,
                             rmw_message_info_t *message_info,
                             rmw_subscription_allocation_t *allocation) {
  if (!real_rmw_take_with_info) {
    real_rmw_take_with_info = dlsym(RTLD_NEXT, "rmw_take_with_info");
  }

  rmw_ret_t ret = real_rmw_take_with_info(subscription, ros_message, taken,
                                          message_info, allocation);
  if (ret == RMW_RET_OK && *taken) {
    modality_after_rmw_take_with_info(subscription, ros_message, message_info);
  }

  return ret;
}

rmw_service_t *rmw_create_service(
    const rmw_node_t *node, const rosidl_service_type_support_t *type_support,
    const char *service_name, const rmw_qos_profile_t *qos_profile) {
  if (!real_rmw_create_service) {
    real_rmw_create_service = dlsym(RTLD_NEXT, "rmw_create_service");
  }

  rmw_service_t *service =
      real_rmw_create_service(node, type_support, service_name, qos_profile);
  if (service != NULL) {
    modality_after_rmw_create_service(node, service, type_support, service_name);
  }

  return service;
}

rmw_ret_t rmw_destroy_service(rmw_node_t *node, rmw_service_t *service) {
  if (!real_rmw_destroy_service) {
    real_rmw_destroy_service = dlsym(RTLD_NEXT, "rmw_destroy_service");
  }

  rmw_ret_t ret = real_rmw_destroy_service(node, service);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_destroy_service((uintptr_t)service);
  }

  return ret;
}

rmw_client_t *rmw_create_client(
    const rmw_node_t *node, const rosidl_service_type_support_t *type_support,
    const char *service_name, const rmw_qos_profile_t *qos_policies) {
  if (!real_rmw_create_client) {
    real_rmw_create_client = dlsym(RTLD_NEXT, "rmw_create_client");
  }

  rmw_client_t *client =
      real_rmw_create_client(node, type_support, service_name, qos_policies);
  if (client != NULL) {
    modality_after_rmw_create_client(node, client, type_support, service_name);
  }

  return client;
}

rmw_ret_t rmw_destroy_client(rmw_node_t *node, rmw_client_t *client) {
  if (!real_rmw_destroy_client) {
    real_rmw_destroy_client = dlsym(RTLD_NEXT, "rmw_destroy_client");
  }

  rmw_ret_t ret = real_rmw_destroy_client(node, client);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_destroy_client((uintptr_t)client);
  }

  return ret;
}

rmw_ret_t rmw_send_request(const rmw_client_t *client, const void *ros_request,
                           int64_t *sequence_id) {
  if (!real_rmw_send_request) {
    real_rmw_send_request = dlsym(RTLD_NEXT, "rmw_send_request");
  }

  rmw_ret_t ret = real_rmw_send_request(client, ros_request, sequence_id);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_send_request(client, ros_request, *sequence_id);
  }

  return ret;
}

rmw_ret_t rmw_take_request(const rmw_service_t *service,
                           rmw_service_info_t *request_header,
                           void *ros_request, bool *taken) {

  if (!real_rmw_take_request) {
    real_rmw_take_request = dlsym(RTLD_NEXT, "rmw_take_request");
  }

  rmw_ret_t ret =
      real_rmw_take_request(service, request_header, ros_request, taken);
  if (ret == RMW_RET_OK && *taken) {
    modality_after_rmw_take_request(service, request_header, ros_request);
  }

  return ret;
}

rmw_ret_t rmw_send_response(const rmw_service_t *service,
                            rmw_request_id_t *request_header,
                            void *ros_response) {

  if (!real_rmw_send_response) {
    real_rmw_send_response = dlsym(RTLD_NEXT, "rmw_send_response");
  }

  rmw_ret_t ret = real_rmw_send_response(service, request_header, ros_response);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_send_response(service, request_header, ros_response);
  }

  return ret;
}

rmw_ret_t rmw_take_response(const rmw_client_t *client,
                            rmw_service_info_t *request_header,
                            void *ros_response, bool *taken) {
  if (!real_rmw_take_response) {
    real_rmw_take_response = dlsym(RTLD_NEXT, "rmw_take_response");
  }

  rmw_ret_t ret =
      real_rmw_take_response(client, request_header, ros_response, taken);
  if (ret == RMW_RET_OK && *taken) {
    modality_after_rmw_take_response(client, request_header, ros_response);
  }

  return ret;
}
